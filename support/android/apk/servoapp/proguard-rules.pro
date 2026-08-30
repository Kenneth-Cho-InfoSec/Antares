# These methods are called by the native Servo embedder through JNI reflection.
# Keep their names and signatures intact when the production app is minified.
-keep class com.krystelligence.antares.engine.AntaresEngineService { *; }
-keep class com.krystelligence.antares.engine.AntaresEngineService$* { *; }
-keep class org.servo.servoview.JNIServo { *; }
-keep class org.servo.servoview.JNIServo$* { *; }
-keep class org.servo.servoview.Servo { *; }
-keep class org.servo.servoview.Servo$* { *; }
