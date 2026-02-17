import edu.lums.impact.Rectangle;
import edu.lums.impact.Shape;

public class RectangleBehaviorTest {
    public static void main(String[] args) {
        Rectangle rect = new Rectangle(3, -2, 7, 9);

        if (!(rect instanceof Shape)) {
            throw new AssertionError("Rectangle should extend Shape");
        }
        if (rect.x != 3 || rect.y != -2) {
            throw new AssertionError("Rectangle should store provided x/y coordinates");
        }
        if (rect.width != 7 || rect.height != 9) {
            throw new AssertionError("Rectangle should expose width/height fields");
        }
        if (rect.getArea() != 63.0) {
            throw new AssertionError("Area should equal width*height for positive dimensions");
        }

        Rectangle negative = new Rectangle(0, 0, -5, 4);
        if (negative.getArea() != -20.0) {
            throw new AssertionError("Area should allow negative dimensions without clamping");
        }
    }
}
